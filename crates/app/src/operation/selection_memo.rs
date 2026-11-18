//! The selection walk's display answers memoized per input (#2248,
//! #2249): the Geometry tab asks for the selection account and the
//! Contour fields for Auto's side on every frame, and each ask ran the
//! whole walk ([`selection_geometry_impl`]) again. Our own design notes
//! (`docs/decisions/2238-frame-cost-rule.md`); no vendor material
//! consulted.
//!
//! The key is the input itself (rule 2): the setup, the picks and the
//! parameters by value, and every world read the walk makes - for each
//! picked entity and the setup's stock, the entity, its facet allocation
//! (an immutable `Arc` every landing replaces), its pose and whether a
//! setup alignment moves it; for the setup, its definition, its machine
//! kind, its WCS pose and word, and the document unit. A hit is the
//! answer a fresh walk gives. Generation, the verify check and the pick
//! doors never read this memo (rule 6): they walk for themselves.

use std::cell::{Cell, RefCell};
use std::sync::{Arc, Weak};

use flecs_ecs::prelude::Entity;

use super::*;
use crate::machine::Kind;
use crate::schema::{Location, SetupDef};
use crate::units::Unit;

/// How many inputs the memo remembers: the open operation, the boundary
/// summary's twin ask and a few recent picks.
const CAP: usize = 8;

/// A facet allocation, compared by address. The held `Weak` keeps the
/// address from being reused while the entry lives.
struct Tris(Weak<[Triangle]>);

impl PartialEq for Tris {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

/// One entity the walk reads, as the world holds it now.
#[derive(PartialEq)]
struct Body {
    name: String,
    entity: Option<Entity>,
    tris: Option<Tris>,
    pose: Option<Location>,
    part: bool,
}

/// Every world read of the walk, taken without walking.
#[derive(PartialEq)]
struct Reads {
    effective: Option<String>,
    def: Option<SetupDef>,
    kind: Kind,
    wcs: Option<Location>,
    wcs_word: String,
    unit: Unit,
    bodies: Vec<Body>,
}

impl Reads {
    fn of(world: &World, setup: Option<&str>, items: &[GeoSel]) -> Self {
        let effective = crate::setup::effective(world, setup);
        let def = effective.as_deref().and_then(|s| crate::setup::def_of(world, s));
        let mut names: Vec<&str> = Vec::new();
        for g in items {
            if !names.contains(&g.entity.as_str()) {
                names.push(&g.entity);
            }
        }
        let stock = crate::setup::stock_entity_name(world, setup);
        if !names.contains(&stock.as_str()) {
            names.push(&stock);
        }
        let bodies = names
            .into_iter()
            .map(|name| {
                let entity = model::find_by_name(world, name);
                Body {
                    name: name.to_owned(),
                    entity,
                    tris: entity.and_then(|e| model::facets_of(world, &e)).map(|t| Tris(Arc::downgrade(&t))),
                    pose: entity.and_then(|e| model::transform_of(world, &e)),
                    part: entity.is_some_and(|e| crate::setup_align::is_part(world, &e)),
                }
            })
            .collect();
        Self {
            effective,
            def,
            kind: crate::setup_mount::kind_of_setup(world, setup),
            wcs: crate::setup::wcs_entity(world, setup).and_then(|e| model::transform_of(world, &e)),
            wcs_word: crate::setup::wcs_name(world, setup),
            unit: crate::units::of(world),
            bodies,
        }
    }
}

/// What the display asks of one walk.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Answer {
    /// The walk's account of what the selection resolved to.
    pub(super) notes: Vec<String>,
    /// The early-door refusal the walk found (`door_refusal`).
    pub(super) refusal: Option<String>,
    /// Auto's side for a Contour: `Some(true)` inside, `None` when there
    /// is no verdict to quote (see [`contour_auto_side`]).
    pub(super) inside: Option<bool>,
}

impl Answer {
    /// One real walk - what every miss runs, and what the pins compare a
    /// hit against.
    pub(super) fn fresh(world: &World, setup: Option<&str>, items: &[GeoSel], params: &OpParams) -> Self {
        let mut notes = Vec::new();
        match selection_geometry_impl(world, setup, items, params, Some(&mut notes)) {
            Ok((loops, _runs, auto)) => {
                let inside = match auto {
                    _ if loops.is_empty() => None,
                    machining2d::AutoSide::Inside => Some(true),
                    machining2d::AutoSide::Outside => Some(false),
                    // No verdict to quote: generation refuses it by name.
                    machining2d::AutoSide::Undecided(_) => None,
                };
                Self { notes, refusal: None, inside }
            }
            Err(err) => Self { notes, refusal: super::picks::door_refusal(params, err), inside: None },
        }
    }
}

struct Entry {
    setup: Option<String>,
    items: Vec<GeoSel>,
    params: OpParams,
    reads: Reads,
    answer: Answer,
}

thread_local! {
    static MEMO: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    /// Real walks this memo ran on this thread - the counter the
    /// #2248/#2249 pins read.
    static WALKS: Cell<u64> = const { Cell::new(0) };
}

/// [`Answer::fresh`], memoized by its full input.
pub(super) fn answer(world: &World, setup: Option<&str>, items: &[GeoSel], params: &OpParams) -> Answer {
    let reads = Reads::of(world, setup, items);
    let hit = MEMO.with(|m| {
        let mut m = m.borrow_mut();
        let i = m.iter().position(|e| {
            e.setup.as_deref() == setup && e.items == items && e.params == *params && e.reads == reads
        })?;
        let entry = m.remove(i);
        let answer = entry.answer.clone();
        m.push(entry); // MRU bump
        Some(answer)
    });
    if let Some(answer) = hit {
        return answer;
    }
    WALKS.with(|n| n.set(n.get().saturating_add(1)));
    let answer = Answer::fresh(world, setup, items, params);
    MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= CAP {
            m.remove(0);
        }
        m.push(Entry {
            setup: setup.map(str::to_owned),
            items: items.to_vec(),
            params: params.clone(),
            reads,
            answer: answer.clone(),
        });
    });
    answer
}

/// How many real walks the display memo ran on this thread
/// ([`selection_account`] and [`contour_auto_side`] together).
#[must_use]
pub fn selection_walks() -> u64 {
    WALKS.with(Cell::get)
}

/// [`contour_auto_side`] without the memo: one real walk. For the pins
/// that prove a hit equals a fresh walk; generation never needs it (it
/// reads the side from its own walk in `job`).
#[must_use]
pub fn contour_auto_side_fresh(world: &World, setup: Option<&str>, src: &SrcRef, params: &OpParams) -> Option<bool> {
    match src {
        SrcRef::Stock | SrcRef::Entity(_) => Some(false),
        SrcRef::Selection(items) => Answer::fresh(world, setup, items, params).inside,
    }
}

/// [`selection_account`] without the memo: one real walk, for the pins.
#[must_use]
pub fn selection_account_fresh(world: &World, setup: Option<&str>, items: &[GeoSel], params: &OpParams) -> (Vec<String>, Option<String>) {
    if let Some(account) = super::probe_account(world, items, params) {
        return account;
    }
    let a = Answer::fresh(world, setup, items, params);
    (a.notes, a.refusal)
}

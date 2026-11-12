//! The carried body's cache (#321): the body a later setup is entered
//! with is the costliest read of a carried setup (every cut ahead swept
//! through every turn of the chain), and it is read again by each op's
//! generation, its rest mask and the post gate's workholding clause. It is
//! computed once per distinct input and served from here until an input
//! moves.
//!
//! Provenance: our own design (content-addressed memo, the M1.5
//! `features_of_local` precedent); no vendor material consulted.
//!
//! ## The laws here
//!
//! - **Keyed by everything the sweep reads, never by a name.** The key
//!   digests the origin setup's stock (placement, mesh signature, shape -
//!   `write_setup_stock`), the lattice radius, every turn of the chain,
//!   and every cutting atom's exact moves and tool atom, in order. What
//!   the sweep does not read (op names, feeds of ops not ahead, the
//!   consumer setup's own ops when only the entered body is asked for,
//!   the viewport) never enters it, so an unrelated edit is a hit - and
//!   anything the sweep does read moves the key, so a stale body is never
//!   served. An input the key cannot digest (a stock with no geometry
//!   signature) is never cached: computed fresh every time.
//! - **Pure fuel.** A hit returns exactly what a miss would compute; no
//!   verdict reads the cache for truth. [`computes`] counts the misses
//!   (test-visible: the pin that an unrelated edit does not recompute).
//! - **Bounded, freed with the document.** Thread-local, most recent
//!   [`CAP`] bodies; opening a document or making a new world clears it
//!   ([`clear`]), so nothing of a closed document outlives it.

use std::cell::{Cell, RefCell};
use std::hash::Hasher;
use std::sync::Arc;

use super::across::Chain;
use crate::cl::{Move, MoveKind};
use crate::stock_sim::Body;
use flecs_ecs::prelude::World;

/// How many carried bodies are kept (the most recently used).
pub const CAP: usize = 4;

thread_local! {
    static CACHE: RefCell<Vec<(u64, Arc<Body>)>> = const { RefCell::new(Vec::new()) };
    static COMPUTES: Cell<u64> = const { Cell::new(0) };
}

/// How many carried bodies this thread has computed (cache misses and
/// uncacheable reads) - the counter the cache's pins read.
#[must_use]
pub fn computes() -> u64 {
    COMPUTES.with(Cell::get)
}

/// Drop every cached body (a document replaced or a new world made).
pub fn clear() {
    CACHE.with(|c| c.borrow_mut().clear());
}

fn f(h: &mut impl Hasher, v: f64) {
    h.write_u64(v.to_bits());
}

fn write_move(h: &mut impl Hasher, m: &Move) {
    h.write_u8(match m.kind {
        MoveKind::Rapid => 0,
        MoveKind::Feed => 1,
        MoveKind::Plunge => 2,
    });
    for v in [m.from.x, m.from.y, m.from.z, m.to.x, m.to.y, m.to.z] {
        f(h, v);
    }
    match &m.arc {
        // Debug prints every float round-trip exact, and the plane by name.
        Some(a) => h.write(format!("{a:?}").as_bytes()),
        None => h.write_u8(0xa0),
    }
    match m.axis {
        Some(a) => a.iter().for_each(|v| f(h, *v)),
        None => h.write_u8(0xa1),
    }
    match m.rotary {
        Some(c) => f(h, c),
        None => h.write_u8(0xa2),
    }
    match m.feed {
        Some(v) => f(h, v),
        None => h.write_u8(0xa3),
    }
}

/// The key of the body `chain` leaves: `None` when an input cannot be
/// digested (then the body is never cached).
fn key(world: &World, chain: &Chain, min_r: f64, through_consumer: bool) -> Option<u64> {
    let mut h = std::hash::DefaultHasher::new();
    h.write(b"camcraft-carried-body-v1");
    let origin = crate::setup::effective(world, chain.origin());
    crate::operation::write_setup_stock(world, origin.as_deref(), &mut h)?;
    f(&mut h, min_r);
    h.write_u8(u8::from(through_consumer));
    // #637: a body is the engine's own read; the column key is unchanged.
    if let crate::stock_sim::Backend::Field { band } = crate::stock_sim::carry_engine() {
        h.write(b"field");
        h.write_u32(band);
    }
    let last = chain.segments.len().saturating_sub(1);
    for (i, s) in chain.segments.iter().enumerate() {
        h.write_u8(0xb0);
        match &s.turn {
            Some(t) => h.write(format!("{t:?}").as_bytes()),
            None => h.write_u8(0),
        }
        if i == last && !through_consumer {
            break;
        }
        for a in s.ops.iter().filter(|a| a.cut) {
            h.write_u8(0xb1);
            f(&mut h, a.radius);
            h.write_u8(u8::from(a.ball));
            for v in [a.cone, a.corner] {
                match v {
                    Some(c) => f(&mut h, c),
                    None => h.write_u8(0),
                }
            }
            h.write_usize(a.moves.len());
            a.moves.iter().for_each(|m| write_move(&mut h, m));
        }
    }
    Some(h.finish())
}

/// The body `chain` leaves, from the cache when its key is held, else
/// computed by `compute` (and kept, when the key could be taken).
///
/// # Errors
/// `compute`'s own; a failed sweep is never cached.
pub(super) fn body(
    world: &World,
    chain: &Chain,
    min_r: f64,
    through_consumer: bool,
    compute: impl FnOnce() -> Result<Body, String>,
) -> Result<Arc<Body>, String> {
    let k = key(world, chain, min_r, through_consumer);
    if let Some(k) = k
        && let Some(hit) = CACHE.with(|c| {
            let mut c = c.borrow_mut();
            let i = c.iter().position(|(kk, _)| *kk == k)?;
            let entry = c.remove(i);
            let out = entry.1.clone();
            c.push(entry); // most recently used last
            Some(out)
        })
    {
        return Ok(hit);
    }
    COMPUTES.with(|n| n.set(n.get().saturating_add(1)));
    let b = Arc::new(compute()?);
    if let Some(k) = k {
        CACHE.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() >= CAP {
                c.remove(0);
            }
            c.push((k, b.clone()));
        });
    }
    Ok(b)
}

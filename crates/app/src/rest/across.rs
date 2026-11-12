//! Rest across setups (#318): the program ahead of an operation, cut
//! into per-setup segments, reaching back through every setup boundary
//! the machined body is CARRIED across (#317's `stock_sim::carry`).
//!
//! Provenance: our own design per #308's research decision ("stock from
//! the preceding setup" read through the setup transform); general
//! machining knowledge, no vendor material consulted.
//!
//! ## The laws here
//!
//! - **One law with the program sim.** A boundary counts exactly when the
//!   program sim carries the body across it (`carry::entry` decides, by
//!   call), so the setup's stock source (#320) decides: "What <setup>
//!   leaves behind" extends the chain; "Its own stock" breaks it, both
//!   in the same hold (the M3.2 A clip) and in a different hold (a new
//!   block) - its rest starts from its own block, the side that never
//!   assumes material gone. Extending the chain into a new blank would
//!   read cuts that blank never had: gouge-class.
//! - **A refused carry refuses the rest.** If the body cannot be carried
//!   into the consumer's setup (a turn the lattice cannot hold, a lathe),
//!   the stock that setup starts from is unknown: the rest reading is
//!   refused by name ([`REST_CARRY_REFUSAL`]), never read as a fresh block.
//! - **A single setup reads exactly as before.** With no carried boundary
//!   the chain is the same-setup walk and no extra term is written, so
//!   single-setup stamps and paths are bit-for-bit unchanged.

use super::ahead::{all_ahead, Ahead};
use crate::stock_sim::carry::{self, Entry};
use crate::geometry::Point3;
use crate::stock_sim::{Body, Dexel, LatticeTurn, SimOp};
use std::sync::Arc;
use flecs_ecs::prelude::World;

/// An axis-aligned box, `(min, max)`.
pub type Bounds = (Point3, Point3);

/// The refusal for a rest reading whose incoming stock cannot be carried.
pub const REST_CARRY_REFUSAL: &str = "this operation reads the stock earlier setups left, and that stock cannot be carried into its setup";

/// The operations of one setup in the chain, and the turn the body takes
/// entering it (`None` for the setup the chain starts on).
pub(super) struct Segment {
    pub(super) setup: Option<String>,
    pub(super) turn: Option<LatticeTurn>,
    pub(super) ops: Vec<Ahead>,
}

/// The program ahead of one operation as its rest reads it.
pub(super) struct Chain {
    pub(super) segments: Vec<Segment>,
}

impl Chain {
    /// Does the chain reach back across a carried boundary?
    pub(super) fn crosses(&self) -> bool {
        self.segments.len() > 1
    }

    /// The setup whose block the chain starts on.
    pub(super) fn origin(&self) -> Option<&str> {
        self.segments.first().and_then(|s| s.setup.as_deref())
    }

    pub(super) fn ops(&self) -> impl Iterator<Item = &Ahead> {
        self.segments.iter().flat_map(|s| s.ops.iter())
    }

    pub(super) fn into_ops(self) -> Vec<Ahead> {
        self.segments.into_iter().flat_map(|s| s.ops).collect()
    }

    /// The chain's own digest term, beside the predecessors' paths: the
    /// setups crossed, each turn, and the origin setup's stock (the block
    /// the carried body was cut from). Only written when the chain crosses.
    pub(super) fn term(&self, world: &World, h: &mut impl std::hash::Hasher) {
        h.write(b"camcraft-rest-across-v1");
        for s in &self.segments {
            h.write(s.setup.as_deref().unwrap_or("").as_bytes());
            h.write_u8(0);
            match &s.turn {
                // Debug prints every float round-trip exact.
                Some(t) => h.write(format!("{t:?}").as_bytes()),
                None => h.write_u8(0),
            }
        }
        if crate::operation::write_setup_stock(world, self.origin(), h).is_none() {
            h.write_u8(0xfe);
        }
    }
}

/// The chain ahead of `name` (consumer setup `setup`).
///
/// # Errors
/// [`REST_CARRY_REFUSAL`] with the carry's own reason (naming both
/// setups) when a boundary the chain must cross refuses.
pub(super) fn chain_of(world: &World, name: &str, setup: Option<&str>, with_moves: bool) -> Result<Chain, String> {
    let consumer = crate::setup::effective(world, setup);
    let mut segments: Vec<Segment> = Vec::new();
    for a in all_ahead(world, name, with_moves) {
        match segments.last_mut() {
            Some(s) if s.setup == a.setup => s.ops.push(a),
            _ => segments.push(Segment { setup: a.setup.clone(), turn: None, ops: vec![a] }),
        }
    }
    if segments.last().is_none_or(|s| s.setup != consumer) {
        segments.push(Segment { setup: consumer, turn: None, ops: Vec::new() });
    }
    // Back from the consumer: every carried boundary extends the chain,
    // the first clip or new block (#320, "Its own stock") ends it.
    let mut start = segments.len() - 1;
    while start > 0 {
        let (from, to) = (segments[start - 1].setup.as_deref(), segments[start].setup.as_deref());
        match carry::entry(world, from, to) {
            Ok(Entry::Clip | Entry::NewBlock) => break,
            Ok(Entry::Carry(t)) => {
                segments[start].turn = Some(t);
                start -= 1;
            }
            Err(e) => return Err(format!("{REST_CARRY_REFUSAL}: {e}")),
        }
    }
    let segments = segments.split_off(start);
    Ok(Chain { segments })
}

/// The body the crossing chain leaves standing, in the consumer's frame:
/// the origin setup's block, each segment's cut moves swept, the body
/// carried at every boundary. Its tops are read as the rest grid - a
/// column cut through on its far side still reads its standing top (more
/// material, never less). Served from the carried-body cache (#321) when
/// nothing it reads has moved.
/// `through_consumer: false` stops at the body the consumer's setup is
/// ENTERED with (#319): its own setup's earlier cuts are not swept.
pub(super) fn carried_body(world: &World, chain: &Chain, min_r: f64, through_consumer: bool) -> Result<Arc<Body>, String> {
    super::carried_cache::body(world, chain, min_r, through_consumer, || sweep(world, chain, min_r, through_consumer))
}

/// [`carried_body`]'s tops: the rest grid.
pub(super) fn carried_grid(world: &World, chain: &Chain, min_r: f64, through_consumer: bool) -> Result<Dexel, String> {
    carried_body(world, chain, min_r, through_consumer).map(|b| b.dex.clone())
}

/// The sweep itself, uncached.
fn sweep(world: &World, chain: &Chain, min_r: f64, through_consumer: bool) -> Result<Body, String> {
    let origin = crate::setup::effective(world, chain.origin());
    // #637: the carried stock on this thread's engine (the column body by
    // default); the body it leaves is read back once, at the end.
    let mut body = carry::first_carried(world, origin.as_deref(), min_r)?;
    let last = chain.segments.len().saturating_sub(1);
    for (i, s) in chain.segments.iter().enumerate() {
        if let Some(t) = &s.turn {
            body = body.carry(t)?;
        }
        if i == last && !through_consumer {
            break;
        }
        let ops: Vec<SimOp> = s
            .ops
            .iter()
            .filter(|a| a.cut)
            .map(|a| SimOp { moves: &a.moves, radius: a.radius, ball: a.ball, cone: a.cone, corner: a.corner })
            .collect();
        body.simulate(&ops).map_err(|e| e.to_string())?;
    }
    body.into_body()
}

/// The block a later carried setup is entered with, bounded (#319): the
/// origin setup's stock box carried through every turn of the chain,
/// uncut. Cuts only remove material, so the real incoming body never
/// stands outside it - the cheap superset, read with no simulation.
/// `None` when the chain crosses no carried boundary.
pub(super) fn carried_block(world: &World, chain: &Chain) -> Result<Option<(Point3, Point3)>, String> {
    if !chain.crosses() {
        return Ok(None);
    }
    let origin = crate::setup::effective(world, chain.origin());
    let b = crate::model::stock_world_aabb_for(world, origin.as_deref()).ok_or_else(|| {
        format!("setup '{}' has no stock to carry", origin.as_deref().unwrap_or_default())
    })?;
    let mut pts: Vec<Point3> = (0..8)
        .map(|i| {
            Point3::new(
                [b.min.x, b.max.x][(i >> 2) & 1],
                [b.min.y, b.max.y][(i >> 1) & 1],
                [b.min.z, b.max.z][i & 1],
            )
        })
        .collect();
    for t in chain.segments.iter().filter_map(|s| s.turn.as_ref()) {
        for p in &mut pts {
            *p = t.apply(*p);
        }
    }
    Ok(bounds_of(&pts))
}

/// The incoming body's own bounds (#319): the cells the carried grid
/// still holds material in (their whole squares) and the highest top,
/// clamped to `block` - both are supersets of the real body, so their
/// intersection is one too. `None` when the grid holds nothing finite.
/// The grid rides along: the engagement law reads its columns.
pub(super) fn carried_body_bounds(
    world: &World,
    chain: &Chain,
    min_r: f64,
    block: (Point3, Point3),
) -> Result<Option<(Bounds, Dexel)>, String> {
    let body = carried_body(world, chain, min_r, false)?;
    let dex = &body.dex;
    // #586: the bounds read through the column seam (M7.3).
    let cols = crate::stock_sim::Columns::of_grid(dex);
    let h = cols.cell() * 0.5;
    let mut pts = Vec::new();
    for iy in 0..cols.ny() {
        for ix in 0..cols.nx() {
            let t = cols.top_at(ix, iy);
            if !t.is_finite() {
                continue;
            }
            let c = cols.center(ix, iy);
            pts.push(Point3::new(c.x - h, c.y - h, t));
            pts.push(Point3::new(c.x + h, c.y + h, t));
        }
    }
    let Some((lo, hi)) = bounds_of(&pts) else { return Ok(None) };
    let (b0, b1) = block;
    let lo = Point3::new(lo.x.max(b0.x), lo.y.max(b0.y), b0.z);
    let hi = Point3::new(hi.x.min(b1.x), hi.y.min(b1.y), hi.z.min(b1.z));
    Ok((lo.x <= hi.x && lo.y <= hi.y && lo.z <= hi.z).then_some(((lo, hi), dex.clone())))
}

/// The axis-aligned bounds of finite points; `None` for none.
fn bounds_of(pts: &[Point3]) -> Option<(Point3, Point3)> {
    let mut lo = Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut hi = Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in pts.iter().filter(|p| p.x.is_finite() && p.y.is_finite() && p.z.is_finite()) {
        lo = Point3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Point3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    (lo.x <= hi.x && lo.y <= hi.y && lo.z <= hi.z).then_some((lo, hi))
}

/// The block setup `setup` is entered with when the program ahead of
/// `name` carries the body into it (#319), with the origin setup's name:
/// [`carried_block`]'s cheap superset. `Ok(None)`: no carried boundary.
///
/// # Errors
/// [`REST_CARRY_REFUSAL`] with the carry's reason, or an origin setup
/// with no stock - the stock this setup starts from is then unknown.
pub fn carried_stock_block(world: &World, name: &str, setup: Option<&str>) -> Result<Option<(String, Bounds)>, String> {
    let chain = chain_of(world, name, setup, false)?;
    let origin = chain.origin().unwrap_or_default().to_owned();
    Ok(carried_block(world, &chain)?.map(|b| (origin, b)))
}

/// The incoming body's bounds, clamped to `block` ([`carried_body_bounds`]),
/// the lattice resolving `tool_r` and every atom ahead.
///
/// # Errors
/// A refused carry, or a body the grid cannot cut - the caller keeps
/// `block`, the superset.
pub fn carried_stock_body(
    world: &World,
    name: &str,
    setup: Option<&str>,
    tool_r: f64,
    block: (Point3, Point3),
) -> Result<Option<(Bounds, Dexel)>, String> {
    let chain = chain_of(world, name, setup, true)?;
    if !chain.crosses() {
        return Ok(None);
    }
    let min_r = chain.ops().filter(|a| a.cut && a.radius > 0.0).map(|a| a.radius).fold(tool_r, f64::min);
    carried_body_bounds(world, &chain, min_r, block)
}

/// The body setup `setup` is entered with when the program ahead of
/// `name` carries it there (#321: the workholding clause reads it), with
/// the origin setup's name - the carried-body cache's own entry for the
/// same chain and lattice the op's generation read. `Ok(None)`: no
/// carried boundary.
///
/// # Errors
/// A refused carry, or a body the grid cannot cut.
pub fn entered_body(
    world: &World,
    name: &str,
    setup: Option<&str>,
    tool_r: f64,
) -> Result<Option<(String, Arc<Body>)>, String> {
    let chain = chain_of(world, name, setup, true)?;
    if !chain.crosses() {
        return Ok(None);
    }
    let min_r = chain.ops().filter(|a| a.cut && a.radius > 0.0).map(|a| a.radius).fold(tool_r, f64::min);
    let origin = chain.origin().unwrap_or_default().to_owned();
    carried_body(world, &chain, min_r, false).map(|b| Some((origin, b)))
}

/// The rest digest of an operation whose verdict does not otherwise read
/// the program ahead (a posed op, #319): written exactly when its setup
/// is entered with a carried body (or the carry refuses), because its
/// judged stock then depends on the origin stock, the turns and the cuts
/// ahead. `None` in every other case, so no other stamp churns.
#[must_use]
pub fn carried_digest(world: &World, name: &str, setup: Option<&str>) -> Option<u64> {
    match chain_of(world, name, setup, false) {
        Ok(c) if !c.crosses() => None,
        _ => super::prior_digest(world, name, setup),
    }
}

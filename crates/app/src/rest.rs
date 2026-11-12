//! Re-MACHining: what the previous operations already cut (M3.1 stage D).
//!
//! Provenance: our own design; tool-vs-tool rest roughing is standard
//! practice, specced from general machining knowledge - no vendor
//! implementation inspected. The grid arithmetic is all
//! [`crate::stock_sim`]'s; this module decides only who counts as
//! *before* an operation and what that means for its stamp.
//!
//! ## The laws
//!
//! - **Program order is the post's order - by CALL, not by reference.**
//!   [`crate::setup::operation_order`]: setup rank, then operation position
//!   within a setup - the very function `posting::program_ops` calls.
//!   What the machine will do first IS what was cut first; the sort's
//!   known wart (an operation named `Operation10` sorts before
//!   `Operation2` inside one setup) is shared so re-machining and the
//!   machine can never disagree.
//! - **Rest crosses setups where the body is carried (#318).** A
//!   predecessor in an earlier setup is ahead exactly when the program
//!   sim carries the machined body from its setup into the consumer's
//!   (#317, `across`): the rest grid is then the origin block cut and
//!   carried through every boundary. A setup entered by the clip (same
//!   hold, not linked) starts from its own block - the side that never
//!   assumes material gone; a refused carry refuses the rest by name.
//! - **The mask only removes cuts, never adds them.** The rest state is
//!   the stock grid with every earlier operation's CACHED moves
//!   subtracted; a consumer's level targets are intersected with it, so
//!   a masked run is a subset of the unmasked one. Skipping a column
//!   whose material stands at or below the plane cuts air at worst -
//!   never a gouge. The model's own islands law keeps guarding the
//!   part, mask or no mask.
//! - **The digest reads the path on record, never an intention.** A
//!   predecessor's term is a hash of its CACHED moves, so an edited
//!   but unregenerated operation moves nobody: its parameters changed,
//!   but the path the program would post has not - and that is what
//!   the dependent's rest is computed from. That combination can never
//!   post anyway (`program_ops` refuses any Stale op in the program),
//!   so the dependent is asked to describe the program exactly as it
//!   would ship today. When the earlier op IS regenerated its cache
//!   moves, the dependent's rest digest moves with it, and the
//!   dependent reads Stale: one `RegenerateAll` converges, its rounds
//!   re-reading each predecessor's new path (M3.1 C, #145). (A recorded `stamp` term was the first design; the stamp
//!   digests the parameters, and `EditOperationParams` drops it to
//!   None - an edit would have marked every dependent Stale for a path
//!   that had not moved.)
//! - **Inert data must not staleness-float (AGENTS.md, M1.6).** The
//!   digest is written only when the operation has predecessors and
//!   only into `Pocket3D` stamps: a first operation's stamp is
//!   bit-for-bit what it was before stage D, and an operation type
//!   that reads no mask digests none - a `Face` op is no more affected
//!   by an earlier pocket than it was yesterday.
//! - **Never-generated predecessors cut nothing** and contribute only
//!   their marker; the moment one generates, the dependent's digest
//!   moves and it goes Stale - the same one-way ratchet the whole
//!   associativity chain runs on.

mod across;
mod ahead;
mod carried_cache;
mod pilots;

pub use pilots::{drills_ahead, turned_ahead, DrillAhead, TurnedAhead};

pub use across::{REST_CARRY_REFUSAL, carried_digest, carried_stock_block, carried_stock_body, entered_body};
pub use carried_cache::{CAP as CARRIED_CACHE_CAP, clear as clear_carried_cache, computes as carried_body_computes};
use across::{carried_grid, chain_of};
use ahead::{ahead_of, digest_of, Ahead};
use crate::cl::Move;
use crate::machining2d::GenError;
use crate::stock_sim::carry::Held;
use crate::stock_sim::{self, Dexel, SimOp};
use flecs_ecs::prelude::World;

/// What the program in front of one operation contributes: the grid it
/// leaves standing (`None` when nothing before it has cut, so no mask
/// is needed) and the digest of everything the mask depends on
/// (`None` when there are no predecessors at all - the first-operation
/// case, whose stamp must not churn).
pub struct PriorState {
    pub grid: Option<Dexel>,
    pub digest: Option<u64>,
}


/// The cheap half of the story, for the stamp: `Some(digest)` exactly
/// when this operation has predecessors whose work it may consume.
/// Reads names, stamps and tool diameters - no grid, no mesh, so
/// `status_of` can ask every frame (M3.1 C's live-questions law).
#[must_use]
pub fn prior_digest(world: &World, name: &str, setup: Option<&str>) -> Option<u64> {
    use std::hash::Hasher;
    let chain = match chain_of(world, name, setup, false) {
        Ok(c) => c,
        // A refused carry digests its reason: generation refuses it by
        // name, and the stamp moves the moment the refusal does.
        Err(e) => {
            let mut h = std::hash::DefaultHasher::new();
            h.write(b"camcraft-rest-refused");
            h.write(e.as_bytes());
            return Some(h.finish());
        }
    };
    if !chain.crosses() {
        let ahead = chain.into_ops();
        return (!ahead.is_empty()).then(|| digest_of(&ahead));
    }
    let mut h = std::hash::DefaultHasher::new();
    chain.term(world, &mut h);
    let ahead: Vec<Ahead> = chain.into_ops();
    h.write_u64(digest_of(&ahead));
    Some(h.finish())
}

/// The refusal a rest reading of `name` meets (#318): `Some` when the
/// stock its setup starts from has to be carried across a boundary the
/// carry refuses. Generation refuses every op of that setup (#319).
#[must_use]
pub fn carry_refusal(world: &World, name: &str, setup: Option<&str>) -> Option<String> {
    chain_of(world, name, setup, false).err()
}

/// The suppressed operations (#110) ahead of `name` in its setup that
/// cut something: the program `name` was generated against holds their
/// cut (this walk has no suppressed filter - by design, the mask is
/// what WAS planned), so posting `name` without them would rapid and
/// slot into stock nothing judged. Never-cutting ones (never
/// generated, an empty path, a probe) are not named: they removed no
/// material `name` could have assumed gone.
#[must_use]
pub fn suppressed_cuts_ahead(world: &World, name: &str, setup: Option<&str>) -> Vec<String> {
    // Across every carried setup boundary too (#318); a refused carry
    // refuses generation, so the same-setup walk is what stands then.
    chain_of(world, name, setup, false)
        .map(across::Chain::into_ops)
        .unwrap_or_else(|_| ahead_of(world, name, setup, false))
        .into_iter()
        .filter(|a| a.cut)
        .filter(|a| {
            crate::model::find_by_name(world, &a.name)
                .and_then(|e| crate::model::operation_of(world, &e))
                .is_some_and(|op| op.suppressed)
        })
        .map(|a| a.name)
        .collect()
}

/// The grid the previous operations leave standing, and the digest it
/// came from. `tool_r` is the consumer's own radius: the lattice must
/// resolve its passes, and the predecessors' atoms can only make it
/// finer. Refusals are generation errors with the reason named - a
/// program that rest-machines cannot quietly pretend the stock
/// was readable.
pub fn prior_state(
    world: &World,
    name: &str,
    setup: Option<&str>,
    tool_r: f64,
) -> Result<PriorState, GenError> {
    let chain = chain_of(world, name, setup, true).map_err(GenError::Invalid)?;
    if chain.crosses() {
        // The stock this setup starts from is what the earlier setups
        // left, carried (#318) - a mask even when nothing ahead cut.
        let digest = prior_digest(world, name, setup);
        let min_r = chain.ops().filter(|a| a.cut && a.radius > 0.0).map(|a| a.radius).fold(tool_r, f64::min);
        let grid = carried_grid(world, &chain, min_r, true)
            .map_err(|e| GenError::Invalid(format!("rest from previous setups: {e}")))?;
        return Ok(PriorState { grid: Some(grid), digest });
    }
    let ahead = chain.into_ops();
    if ahead.is_empty() {
        return Ok(PriorState {
            grid: None,
            digest: None,
        });
    }
    let digest = digest_of(&ahead);
    if !ahead.iter().any(|a| a.cut) {
        // Nothing cut before us: the stock as it came is the rest as
        // it stands, and no mask is a cheaper way to say that.
        return Ok(PriorState {
            grid: None,
            digest: Some(digest),
        });
    }
    let min_r = ahead
        .iter()
        .filter(|a| a.cut && a.radius > 0.0)
        .map(|a| a.radius)
        .fold(tool_r, f64::min);
    let refused = |e: String| GenError::Invalid(format!("rest from previous operations: {e}"));
    // #637: the stock on this thread's engine (the column grid by default).
    let mut held = stock_sim::carry::first_held(world, setup, min_r).map_err(refused)?;
    let ops: Vec<SimOp> = ahead
        .iter()
        .filter(|a| a.cut)
        .map(|a| SimOp {
            moves: &a.moves,
            radius: a.radius,
            ball: a.ball,
            cone: a.cone,
            corner: a.corner,
        })
        .collect();
    held.simulate(&ops).map_err(refused)?;
    Ok(PriorState {
        grid: Some(held.into_tops().map_err(refused)?),
        digest: Some(digest),
    })
}

/// The program ahead of one operation, collected but not yet cut
/// (issue #96 review): the stock grid and the predecessors' cached
/// moves with their atoms. Verify's vertical-rapid law needs the rest
/// surface only when it has convicted a dive, so the simulation - the
/// costly half of [`prior_state`] - runs on the job thread, and only
/// then ([`PriorPlan::simulate`]).
pub struct PriorPlan {
    /// #637: the stock on the engine the plan was built with - plain for
    /// a same-setup plan, two-sided carried stock for a crossing one.
    held: Held,
    ahead: Vec<Ahead>,
    /// #318: the setups the body is carried through after `ahead` is
    /// cut - each turn and the cut operations of that setup. Empty for a
    /// same-setup plan.
    carried: Vec<(crate::stock_sim::LatticeTurn, Vec<Ahead>)>,
}

impl PriorPlan {
    /// The grid the plan's operations leave standing; a refusal (a
    /// posed predecessor the column grid cannot cut) is the caller's
    /// "prior unknown" - the raw stock, never an acquittal.
    pub fn simulate(self) -> Result<Dexel, String> {
        let mut held = self.held;
        sweep(&mut held, &self.ahead)?;
        for (turn, ops) in &self.carried {
            held = match held {
                Held::Carried(c) => Held::Carried(c.carry(turn)?),
                // `prior_plan` builds every crossing plan carried.
                Held::Plain(_) => return Err("a plain prior stock cannot be carried between setups".into()),
            };
            sweep(&mut held, ops)?;
        }
        held.into_tops()
    }
}

/// Cut `ahead` into `held`, each through its own atom.
fn sweep(held: &mut Held, ahead: &[Ahead]) -> Result<(), String> {
    {
        // A turning insert cuts with its nose: the centre path lowered
        // to the tip, swept as a ball (issue #89).
        let tips: Vec<Vec<Move>> = ahead
            .iter()
            .map(|a| match a.nose {
                Some(n) => a
                    .moves
                    .iter()
                    // A vertical-plane arc's centre moves with its ends:
                    // lowering only the ends would bend it (review nit).
                    .filter(|m| m.arc.as_ref().is_none_or(|a| a.plane.is_xy()))
                    .map(|m| {
                        let mut m = *m;
                        m.from.z -= n;
                        m.to.z -= n;
                        m
                    })
                    .collect(),
                None => Vec::new(),
            })
            .collect();
        let ops: Vec<SimOp> = ahead
            .iter()
            .zip(&tips)
            .map(|(a, tip)| match a.nose {
                Some(n) => SimOp { moves: tip, radius: n, ball: true, cone: None, corner: None },
                None => SimOp { moves: &a.moves, radius: a.radius, ball: a.ball, cone: a.cone, corner: a.corner },
            })
            .collect();
        held.simulate(&ops)
    }
}

/// The program ahead of `name` in its setup, collected for verify's
/// prior reading: `Ok(None)` when nothing ahead has cut (the raw stock
/// IS the prior), an error when the stock grid cannot be built (the
/// caller treats that as an unknown prior - raw stock).
pub fn prior_plan(world: &World, name: &str, setup: Option<&str>, tool_r: f64) -> Result<Option<PriorPlan>, String> {
    let chain = chain_of(world, name, setup, true)?;
    let crosses = chain.crosses();
    let origin = crate::setup::effective(world, chain.origin());
    let mut segs = chain.segments.into_iter();
    let first: Vec<Ahead> = segs.next().map(|s| s.ops).unwrap_or_default().into_iter().filter(|a| a.cut).collect();
    let carried: Vec<(crate::stock_sim::LatticeTurn, Vec<Ahead>)> = segs
        .filter_map(|s| s.turn.map(|t| (t, s.ops.into_iter().filter(|a| a.cut).collect())))
        .collect();
    let every = || first.iter().chain(carried.iter().flat_map(|(_, o)| o.iter()));
    // A crossing plan is read even when nothing cut: the carried block
    // is not the consumer's own (#318).
    if !crosses && first.is_empty() {
        return Ok(None);
    }
    let ahead: Vec<&Ahead> = every().collect();
    // #89 review (B1/B2): on a lathe the column grid credits a turning
    // move under standing material (a parting kerf, an ID groove behind
    // a lip) with everything above it, and a bar or blade cut as a full
    // disc would be revolved round the bar. Only the Facing kind, cut
    // with its nose, never undercuts: any other kind ahead is unknown.
    // The same holds for a turning kind on a mill-turn: the column grid
    // reads its undercuts the same way, so outside a lathe every turning
    // kind but nose-cut Facing ahead is unknown too.
    let lathe = crate::machine::kind_of_setup(world, setup) == crate::machine::Kind::Lathe;
    let facing = |a: &Ahead| a.kind == crate::operation::OpType::Facing && a.nose.is_some();
    let turning = |a: &Ahead| a.kind.info().family() == crate::operation::Family::Turn;
    if let Some(a) = ahead.iter().find(|a| !facing(a) && (lathe || turning(a))) {
        return Err(format!(
            "operation '{}' ahead is a turning pass other than facing - the column grid cannot read its removal",
            a.name
        ));
    }
    let min_r = ahead.iter().map(|a| a.nose.unwrap_or(a.radius)).filter(|r| *r > 0.0).fold(tool_r, f64::min);
    if !crosses {
        let held = stock_sim::carry::first_held(world, setup, min_r)?;
        return Ok(Some(PriorPlan { held, ahead: first, carried }));
    }
    let held = Held::Carried(stock_sim::carry::first_carried(world, origin.as_deref(), min_r)?);
    Ok(Some(PriorPlan { held, ahead: first, carried }))
}

/// The staleness story re-machining adds to `stale_reason`'s list of
/// live questions - asked LAST, after every question the operation can
/// answer about its own inputs. If the dependent still reads Stale
/// with all of those clean, its rest digest moved, and the only terms
/// that can move are a cutting predecessor's path or atom: a never
/// generated predecessor contributes a constant. So naming the first
/// cutting predecessor IS the honest attribution, whether that
/// operation itself reads Ok (it was generated after the dependent
/// was) or Stale (something ahead moved it, or the batch simply has
/// not run). The order law above is what makes "ahead" mean what the
/// machine means.
pub fn prior_stale(world: &World, name: &str, setup: Option<&str>) -> Option<String> {
    let chain = match chain_of(world, name, setup, false) {
        Ok(c) => c,
        Err(e) => return Some(e),
    };
    let crosses = chain.crosses();
    let origin = chain.origin().map(str::to_owned);
    match chain.into_ops().into_iter().find(|a| a.cut) {
        Some(a) if crosses => Some(format!(
            "operation '{}' ahead of it in the program, or a setup the stock is carried through, changed - regenerate the program in order",
            a.name
        )),
        Some(a) => Some(format!(
            "operation '{}' ahead of it in the program changed - regenerate the program in order",
            a.name
        )),
        None if crosses => Some(format!(
            "the stock carried from setup '{}' changed - regenerate the program in order",
            origin.unwrap_or_default()
        )),
        None => None,
    }
}

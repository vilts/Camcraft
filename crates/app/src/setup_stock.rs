//! Where a setup's stock comes from (#320): its own stock, or what the
//! setup before it leaves behind (the machined body #317 carries).
//!
//! Provenance: our own design; "stock from the preceding setup" is a
//! plain CAM workflow described in public CAM documentation (behaviour
//! only). No vendor implementation inspected.
//!
//! ## The laws here
//!
//! - **A user parameter with a default.** `SetupDef::stock_source` is
//!   `None` until the user picks (every file before v57). `None` reads
//!   as #317's implicit rule ([`implicit`]): a setup that holds the part
//!   differently from the one before it, or is placed relative to it,
//!   takes what that setup leaves; otherwise its own stock. So an
//!   untouched document simulates exactly as it did.
//! - **Own in the same hold is the clip.** "Its own stock" in a setup
//!   holding the part the same way as the one before keeps the M3.2 A
//!   clip (the block bounds what stands; earlier cuts inside it stay
//!   cut). In a different hold it is a new block: the machine holds the
//!   setup's own stock, not the earlier body.
//! - **The first setup has nothing before it.** It always uses its own
//!   stock; no choice is stored or offered.
//! - **A carry that cannot be verified is refused by name** before it can
//!   be chosen ([`carry_refusal`]).

use flecs_ecs::prelude::World;

use crate::setup;
use crate::setup_align;
use crate::setup_relative;

/// The stock a setup starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockSource {
    /// The setup's own stock (its Stock node).
    Own,
    /// The machined stock the previous setup leaves behind.
    FromPrevious,
}

impl StockSource {
    /// The file word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            StockSource::Own => "own",
            StockSource::FromPrevious => "from_previous",
        }
    }

    /// The file word read back; `None` for a word this build does not know.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        match w {
            "own" => Some(StockSource::Own),
            "from_previous" => Some(StockSource::FromPrevious),
            _ => None,
        }
    }
}

/// The setup before `name` in setup order (`None`: the first setup, or
/// no such setup).
#[must_use]
pub fn previous(world: &World, name: &str) -> Option<String> {
    let names = setup::names(world);
    let i = names.iter().position(|n| n == name)?;
    names.get(i.checked_sub(1)?).cloned()
}

/// The source the user picked for `name` (`None`: never picked).
#[must_use]
pub fn stored(world: &World, name: &str) -> Option<StockSource> {
    setup::def_of(world, name).and_then(|d| d.stock_source)
}

/// #317's implicit rule for entering `to` from `from`: linked to it, or
/// holding the part differently, takes what `from` leaves; else own.
#[must_use]
pub fn implicit(world: &World, from: &str, to: &str) -> StockSource {
    let linked = setup_relative::relative_to(world, to).as_deref() == Some(from);
    if linked || !same_hold(world, from, to) {
        StockSource::FromPrevious
    } else {
        StockSource::Own
    }
}

/// Do two setups hold the part the same way?
#[must_use]
pub fn same_hold(world: &World, from: &str, to: &str) -> bool {
    setup_relative::same(setup_align::stored(world, from), setup_align::stored(world, to))
}

/// The source the program uses entering `to` from `from`: the stored
/// pick, else the implicit rule.
#[must_use]
pub fn between(world: &World, from: &str, to: &str) -> StockSource {
    stored(world, to).unwrap_or_else(|| implicit(world, from, to))
}

/// The source `name` uses against the setup before it; `None` for the
/// first setup (always its own stock) or a name that is no setup.
#[must_use]
pub fn effective(world: &World, name: &str) -> Option<StockSource> {
    let prev = previous(world, name)?;
    Some(between(world, &prev, name))
}

/// Why `name` cannot take the stock the setup before it leaves, if it
/// cannot: the first setup, or a carry the sim refuses (a turn the
/// lattice cannot carry exactly, or a machine that is not a mill, named by
/// its kind) - named with both setups.
#[must_use]
pub fn carry_refusal(world: &World, name: &str) -> Option<String> {
    let Some(prev) = previous(world, name) else {
        return Some(format!("'{name}' is the first setup - there is no setup before it to take stock from"));
    };
    crate::stock_sim::carry::carry_turn(world, &prev, name).err()
}

/// The plain words for a setup's stock source: "Its own stock" or
/// "What Setup1 leaves behind". `None` for the first setup.
#[must_use]
pub fn describe(world: &World, name: &str) -> Option<String> {
    let prev = previous(world, name)?;
    Some(match between(world, &prev, name) {
        StockSource::Own => "Its own stock".to_owned(),
        StockSource::FromPrevious => format!("What {prev} leaves behind"),
    })
}

/// What a setup starts from, measured: the box the incoming body fills
/// (min, max) and its volume (mm³).
#[derive(Debug, Clone, PartialEq)]
pub struct Incoming {
    pub min: [f64; 3],
    pub max: [f64; 3],
    pub volume: f64,
}

/// The incoming stock of `name` as the program sim reaches it
/// (`stock_sim::incoming_body`), measured.
///
/// # Errors
/// Every refusal of the program sim, a setup the program never enters,
/// and a body with no material left.
pub fn incoming(world: &World, name: &str) -> Result<Incoming, String> {
    let body = crate::stock_sim::incoming_body(world, name)?;
    let (lo, hi) = body.bounds().ok_or_else(|| format!("no material reaches setup '{name}'"))?;
    Ok(Incoming { min: [lo.x, lo.y, lo.z], max: [hi.x, hi.y, hi.z], volume: body.volume() })
}

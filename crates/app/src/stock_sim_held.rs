//! The stock as the program walk and the rest chains hold it (#637 step
//! E3): a plain `Stock` until the first carry, the engine-agnostic
//! `CarriedStock` after - and which engine carries it.
//!
//! Provenance: our own design (#637's design comments and
//! `docs/decisions/637-carried-stock-holders.md`); the engines are
//! camcraft-sim's. No vendor material consulted.
//!
//! ## The laws here
//!
//! - **The column engine is what ships.** [`engine`] answers `Column`
//!   unless a caller asked otherwise, and on that arm every call here is
//!   the call the holders made before (`stock_grid`, `cut::simulate`,
//!   `deviate`, `Body::carry`): verdicts, goldens and stamps are
//!   unchanged. #589 flips the default.
//! - **The field arm is a closed solid on the column lattice.** It is
//!   built over the very lattice `stock_grid` gives (so a verdict column
//!   is a field cell centre), from the stock's real floor to its top, and
//!   is two-sided from the first cut - there is no plain stage to convert
//!   at the first carry.
//! - **Refuse what is not verified.** A cylinder stock's field would be
//!   its mesh, an inscribed polygon holding less than the column engine's
//!   disc at the rim: refused by name on the field arm
//!   ([`FIELD_CARRY_SHAPE_REFUSAL`]), never read short.
//! - **The engine is a per-thread choice.** [`with_engine`] scopes it to
//!   one call tree (the agreement lane runs both in one process);
//!   `CAMCRAFT_CARRY_ENGINE=field` sets the process default for running
//!   the carried suites whole on the field arm. A job thread never reads
//!   the switch: a plan carries the stock it was built with.

use std::cell::Cell;
use std::sync::OnceLock;

use flecs_ecs::prelude::World;

use crate::geometry::Triangle;
use crate::stock_sim::{
    clip_columns_under, lattice_of_grid, stock_grid, Backend, Body, CarriedStock, Deviation, Dexel, FieldBody, SimOp,
    Stock, FIELD_BAND,
};

/// The environment variable that sets the process's default carry engine
/// (`field`; anything else, or unset, is the column engine).
pub const CARRY_ENGINE_ENV: &str = "CAMCRAFT_CARRY_ENGINE";

/// The field arm's refusal of a cylinder stock (the module law).
pub const FIELD_CARRY_SHAPE_REFUSAL: &str = "the field engine does not carry a cylinder stock yet - its mesh holds less than the round at the rim; the column engine carries it";

/// A plain stock on the field arm: never built (the field arm is carried
/// stock from its first cut), refused by name if it ever is.
const PLAIN_FIELD_REFUSAL: &str = "a plain field stock cannot be carried or clipped - the field arm holds carried stock from the start";

thread_local! {
    static ENGINE: Cell<Option<Backend>> = const { Cell::new(None) };
}

fn default_engine() -> Backend {
    static DEFAULT: OnceLock<Backend> = OnceLock::new();
    *DEFAULT.get_or_init(|| match std::env::var(CARRY_ENGINE_ENV) {
        Ok(v) if v.trim().eq_ignore_ascii_case("field") => Backend::Field { band: FIELD_BAND },
        _ => Backend::Column,
    })
}

/// The engine this thread's carried stock is built on.
#[must_use]
pub fn engine() -> Backend {
    ENGINE.with(Cell::get).unwrap_or_else(default_engine)
}

/// Run `f` with this thread's carried stock on engine `b`; the engine
/// before is restored when `f` returns (or unwinds).
pub fn with_engine<R>(b: Backend, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Backend>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ENGINE.with(|e| e.set(self.0));
        }
    }
    let _restore = Restore(ENGINE.with(|e| e.replace(Some(b))));
    f()
}

/// The stock as the walk holds it: the plain stock until the first
/// carry, the two-sided carried stock after (the field arm from the
/// start).
pub(crate) enum Held {
    Plain(Stock),
    Carried(CarriedStock),
}

/// The first setup's stock read as a body: its floor is the stock's
/// bottom (the box and cylinder from their placement, a from-model
/// stock from its mesh read from below). Upright cuts never move the
/// floor, so it is read when the first carry needs it.
pub(crate) fn first_body(world: &World, setup: Option<&str>, dex: Dexel) -> Result<Body, String> {
    use crate::{model, stock::StockShape};
    let name = crate::setup::stock_entity_name(world, setup);
    let e = model::find_by_name(world, &name).ok_or_else(|| format!("no stock '{name}' to carry"))?;
    let shape = model::stock_def_of(world, &e).ok_or("the stock entity lost its shape")?;
    let loc = model::transform_of(world, &e).unwrap_or_default();
    Ok(match shape {
        StockShape::FromModel { .. } => Body::on_mesh(dex, &model::world_facets(world, &e).unwrap_or_default()),
        _ => Body::on_floor(dex, loc.pos.z),
    })
}

/// The setup's stock as a field body over `dex`'s lattice (the module
/// law).
fn first_field(world: &World, setup: Option<&str>, dex: &Dexel, band: u32) -> Result<FieldBody, String> {
    use crate::{model, stock::StockShape};
    let name = crate::setup::stock_entity_name(world, setup);
    let e = model::find_by_name(world, &name).ok_or_else(|| format!("no stock '{name}' to carry"))?;
    let shape = model::stock_def_of(world, &e).ok_or("the stock entity lost its shape")?;
    let loc = model::transform_of(world, &e).unwrap_or_default();
    let lat = lattice_of_grid(dex);
    match shape {
        StockShape::Box { .. } => FieldBody::block(lat, loc.pos.z, dex.stock_z, band),
        StockShape::FromModel { .. } => FieldBody::mesh(lat, &model::world_facets(world, &e).unwrap_or_default(), band),
        StockShape::Cylinder { .. } => return Err(FIELD_CARRY_SHAPE_REFUSAL.into()),
    }
    .map_err(|e| e.to_string())
}

/// The stock of `setup` as a program first holds it, on this thread's
/// engine, its lattice resolving `min_r`.
///
/// # Errors
/// `stock_grid`'s refusals; the field arm's own (the module law).
pub(crate) fn first_held(world: &World, setup: Option<&str>, min_r: f64) -> Result<Held, String> {
    let dex = stock_grid(world, setup, min_r)?;
    match engine() {
        Backend::Column => Ok(Held::Plain(Stock::Column(dex))),
        Backend::Field { band } => first_field(world, setup, &dex, band).map(|f| Held::Carried(CarriedStock::Field(Box::new(f)))),
    }
}

/// The stock of `setup` as a chain that will carry it first holds it:
/// two-sided from the start, on this thread's engine.
///
/// # Errors
/// As [`first_held`], and a stock entity that cannot be read as a body.
pub(crate) fn first_carried(world: &World, setup: Option<&str>, min_r: f64) -> Result<CarriedStock, String> {
    first_held(world, setup, min_r)?.into_carried(world, setup)
}

impl Held {
    /// Cut `ops` (current frame) into the stock.
    pub(crate) fn simulate(&mut self, ops: &[SimOp]) -> Result<(), String> {
        match self {
            Held::Plain(s) => s.simulate(ops),
            Held::Carried(c) => c.simulate(ops),
        }
        .map_err(|e| e.to_string())
    }

    /// Clip under the entered setup's own stock `grid` (the M3.2 A clip).
    pub(crate) fn clip_under(&mut self, grid: &Dexel) -> Result<(), String> {
        match self {
            Held::Plain(Stock::Column(d)) => clip_columns_under(d, grid),
            Held::Plain(Stock::Field(_)) => return Err(PLAIN_FIELD_REFUSAL.into()),
            Held::Carried(c) => c.clip_under(grid),
        }
        Ok(())
    }

    /// The stock as carried stock: a plain column grid takes its floor
    /// from `origin`'s stock entity ([`first_body`]).
    pub(crate) fn into_carried(self, world: &World, origin: Option<&str>) -> Result<CarriedStock, String> {
        match self {
            Held::Plain(Stock::Column(d)) => first_body(world, origin, d).map(CarriedStock::Column),
            Held::Plain(Stock::Field(_)) => Err(PLAIN_FIELD_REFUSAL.into()),
            Held::Carried(c) => Ok(c),
        }
    }

    /// The verdict of the stock as held, against `facets`.
    pub(crate) fn deviate(&self, facets: &[Triangle], tol: f64) -> Result<Deviation, String> {
        match self {
            Held::Plain(s) => s.deviate(facets, tol).map_err(|e| e.to_string()),
            Held::Carried(c) => c.deviate(facets, tol),
        }
    }

    /// The verdict and the tops the viewer draws (a carried body's
    /// `display`: a column cut through is air, not a sheet).
    pub(crate) fn verdict(self, facets: &[Triangle], tol: f64) -> Result<(Deviation, Dexel), String> {
        match self {
            Held::Plain(Stock::Column(d)) => {
                let dev = crate::stock_sim::deviate(&d, facets, tol).map_err(|e| e.to_string())?;
                Ok((dev, d))
            }
            Held::Plain(Stock::Field(_)) => Err(PLAIN_FIELD_REFUSAL.into()),
            Held::Carried(c) => {
                let body = c.into_body()?;
                let dev = body.deviate(facets, tol).map_err(|e| e.to_string())?;
                Ok((dev, body.display()))
            }
        }
    }

    /// The tops standing, on the stock's lattice: the rest grid.
    pub(crate) fn into_tops(self) -> Result<Dexel, String> {
        match self {
            Held::Plain(Stock::Column(d)) => Ok(d),
            Held::Plain(Stock::Field(_)) => Err(PLAIN_FIELD_REFUSAL.into()),
            Held::Carried(c) => c.into_body().map(|b| b.dex),
        }
    }
}

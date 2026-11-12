//! SetStockSourceOnly (#1094): say whether the active setup's from-model
//! stock's source model is raw stock only (a casting or blank you cut
//! into) - then the program verdict does not judge it as the finished
//! part. Off is the default and every older document's state.
//!
//! Provenance: our own design (a CAM choice is a user parameter with a
//! safe default); the command shape follows `SetStock` - validate and
//! snapshot at construction, pure world writes in apply/invert.

use flecs_ecs::prelude::*;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::StockSourceOnly;
use crate::stock::StockShape;

pub struct SetStockSourceOnly {
    /// The stock entity, resolved from the active setup at construction
    /// (undo replays against this name, as `SetStock` does).
    stock_name: String,
    next: Option<StockSourceOnly>,
    prev: Option<StockSourceOnly>,
}

impl SetStockSourceOnly {
    /// `on` marks the current source model raw stock only; off clears the
    /// mark. Turning it on needs a from-model stock - any other stock has
    /// no source model, and the refusal says so.
    pub fn new(world: &World, on: bool) -> Result<Self, CommandError> {
        let active = crate::setup::active(world);
        let stock_name = crate::setup::stock_entity_name(world, active.as_deref());
        let entity = model::find_by_name(world, &stock_name)
            .filter(|e| model::is_stock(world, e))
            .ok_or_else(|| CommandError::Invalid(format!("no stock '{stock_name}' - set the stock first")))?;
        let prev = entity
            .entity_view(world)
            .try_get::<&StockSourceOnly>(|f| f.clone());
        let next = if on {
            match model::stock_def_of(world, &entity) {
                Some(StockShape::FromModel { source }) => {
                    // The mark carries the geometry it was said of (#1094,
                    // third review): `stock_source` honours it only while
                    // the model still prints the same.
                    let print = model::find_by_name(world, &source)
                        .and_then(|m| crate::stock_print::print_of(world, &m))
                        .ok_or_else(|| CommandError::Invalid(format!(
                            "the stock's source model '{source}' is not in the document or has no geometry"
                        )))?;
                    Some(StockSourceOnly { source, print })
                }
                _ => {
                    return Err(CommandError::Invalid(format!(
                        "'{stock_name}' is not made from a model - only a from-model stock's source can be raw stock only"
                    )));
                }
            }
        } else {
            None
        };
        Ok(SetStockSourceOnly { stock_name, next, prev })
    }
}

fn land(world: &World, stock_name: &str, value: &Option<StockSourceOnly>) {
    let Some(entity) = model::find_by_name(world, stock_name) else {
        return; // the stock is gone (idempotent replay)
    };
    let view = entity.entity_view(world);
    match value {
        Some(f) => {
            view.set(f.clone());
        }
        None => {
            view.remove(world.component::<StockSourceOnly>());
        }
    }
}

impl Command for SetStockSourceOnly {
    fn apply(&self, world: &World) {
        land(world, &self.stock_name, &self.next);
    }

    fn invert(&self, world: &World) {
        land(world, &self.stock_name, &self.prev);
    }
}

/// The marks a command takes away with it (#1094 review): a mark dies
/// with the model it was given to and with the stock it was ticked on.
/// `DeleteEntity` (of the source model, or of the stock), `UpdateModel`
/// (a re-import in place) and `SetStock` snapshot the marks they drop at
/// construction and drop them in `apply`; `invert` puts them back, so one
/// undo restores the model, the stock and the mark together. A later
/// model wearing the old name never finds a mark waiting for it.
#[derive(Default)]
pub(crate) struct MarkDrops(Vec<(String, StockSourceOnly)>);

impl MarkDrops {
    /// Every stock's mark that names the model `name`, and the mark of
    /// the stock entity `name` itself.
    pub(crate) fn of(world: &World, name: &str) -> Self {
        let marks = model::named_entities(world)
            .into_iter()
            .filter(|(_, e)| model::is_stock(world, e))
            .filter_map(|(stock, e)| {
                e.entity_view(world)
                    .try_get::<&StockSourceOnly>(|f| f.clone())
                    .filter(|f| f.source == name || stock == name)
                    .map(|f| (stock, f))
            })
            .collect();
        MarkDrops(marks)
    }

    /// The mark ticked on the stock entity `stock` alone (for `SetStock`).
    pub(crate) fn of_stock(world: &World, stock: &str) -> Self {
        let mark = model::find_by_name(world, stock)
            .and_then(|e| e.entity_view(world).try_get::<&StockSourceOnly>(|f| f.clone()));
        MarkDrops(mark.map(|m| (stock.to_owned(), m)).into_iter().collect())
    }

    /// `true`: drop the marks (apply); `false`: put them back (invert).
    pub(crate) fn write(&self, world: &World, dropped: bool) {
        for (stock, mark) in &self.0 {
            land(world, stock, &(!dropped).then(|| mark.clone()));
        }
    }
}

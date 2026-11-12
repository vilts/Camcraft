//! The one derivation of "fit a built-in vise to the setup's stock"
//! (#1105): `InsertVise` places from it, and the vise dialog shows its
//! outcome (no stock yet, where the vise lands, how far the jaws open, or
//! the refusal) before Place - so the preview and the command can never
//! disagree. Pure over the world's stock box and `vise::fit_to_stock`.
//!
//! Provenance: our own composition of `model::stock_world_aabb_for` and
//! `vise::fit_to_stock`; no outside source.

use flecs_ecs::prelude::World;

use crate::geometry::Point3;
use crate::model;
use crate::vise::{self, ViseParams, ViseStyle};

/// Why a fit could not be planned. Both read in plain English - the text
/// is shown as is, in the dialog and after a refused Place.
#[derive(Clone, Debug, PartialEq)]
pub enum FitRefusal {
    /// The setup's stock has no body yet.
    NoStock(String),
    /// The stock is there but the vise law or its opening refuses it.
    Refused(String),
}

impl FitRefusal {
    /// The refusal's text.
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            FitRefusal::NoStock(m) | FitRefusal::Refused(m) => m,
        }
    }
}

/// How a setup is named in a sentence: "setup 'S'", or "the setup" for
/// the implicit one.
#[must_use]
pub fn setup_phrase(setup: Option<&str>) -> String {
    setup.map_or_else(|| "the setup".to_owned(), |s| format!("setup '{s}'"))
}

/// The plain-English refusal for a setup with no stock body, for fitting
/// `what` ("the vise to", "the parallels under", ...).
#[must_use]
pub fn no_stock_message(setup: Option<&str>, what: &str) -> String {
    format!(
        "{} has no stock yet, so there is nothing to fit {what} - set the stock first (Stock page)",
        setup_phrase(setup)
    )
}

/// Does `setup` (`None` - the implicit one) have a stock body to fit to?
#[must_use]
pub fn has_stock(world: &World, setup: Option<&str>) -> bool {
    model::stock_world_aabb_for(world, setup).is_some()
}

/// Fit vise `params` to `setup`'s stock: the vise (opening set) and the
/// pose of its frame.
pub fn fit(world: &World, params: &ViseParams, setup: Option<&str>) -> Result<(ViseParams, crate::schema::Location), FitRefusal> {
    let stock = model::stock_world_aabb_for(world, setup).ok_or_else(|| {
        FitRefusal::NoStock(format!(
            "{} - or place the vise by position instead",
            no_stock_message(setup, "the vise to")
        ))
    })?;
    vise::fit_to_stock(params, &stock).map_err(FitRefusal::Refused)
}

/// What the dialog says before Place.
#[derive(Clone, Debug, PartialEq)]
pub enum VisePreview {
    /// No stock body: Fit cannot be used.
    NoStock,
    /// The vise would land here with its jaws open this far (mm).
    Fits { opening: f64, at: Point3, centred: bool },
    /// The stock is there but the fit is refused (named).
    Refused(String),
}

/// The preview of [`fit`] for the same arguments - the same call, so the
/// two cannot disagree.
#[must_use]
pub fn preview(world: &World, params: &ViseParams, setup: Option<&str>) -> VisePreview {
    match fit(world, params, setup) {
        Ok((fitted, at)) => VisePreview::Fits {
            opening: fitted.opening,
            at: at.pos,
            centred: matches!(fitted.style, ViseStyle::SelfCentering(_)),
        },
        Err(FitRefusal::NoStock(_)) => VisePreview::NoStock,
        Err(FitRefusal::Refused(m)) => VisePreview::Refused(m),
    }
}
